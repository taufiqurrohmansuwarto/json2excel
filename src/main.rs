use std::collections::HashMap;
use std::convert::Infallible;
use warp::Filter;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use xlsxwriter::*;
use xlsxwriter::prelude::*;
use log::{info, error};

#[derive(Deserialize, Debug)]
struct ExportRequest {
    data: Vec<Value>,
    options: ExportOptions,
}

#[derive(Deserialize, Debug)]
struct ExportOptions {
    filename: String,
    sheet_name: Option<String>,
    headers: Option<Vec<String>>, // Custom headers jika ada
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    message: String,
    records_processed: Option<usize>,
    processing_time_ms: Option<u128>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    service: String,
    version: String,
}

// Main handler untuk generate Excel
async fn generate_excel_handler(req: ExportRequest) -> Result<impl warp::Reply, warp::Rejection> {
    let start_time = std::time::Instant::now();
    
    info!("🦀 Starting Excel generation for {} records", req.data.len());
    
    match generate_excel_file(req).await {
        Ok(excel_data) => {
            let duration = start_time.elapsed();
            info!("✅ Excel generated successfully in {:?}", duration);
            
            Ok(warp::reply::with_header(
                excel_data,
                "content-type",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ))
        }
        Err(e) => {
            error!("❌ Excel generation failed: {}", e);
            Err(warp::reject::custom(ExcelError::GenerationFailed(e.to_string())))
        }
    }
}

// Core function untuk generate Excel
async fn generate_excel_file(req: ExportRequest) -> anyhow::Result<Vec<u8>> {
    let sheet_name = req.options.sheet_name.unwrap_or_else(|| "Sheet1".to_string());
    
    // Create workbook - temporarily write to file
    let temp_file = format!("/tmp/temp_{}.xlsx", uuid::Uuid::new_v4());
    
    info!("📝 Creating workbook with sheet: {}", sheet_name);
    let workbook = Workbook::new(&temp_file)?;
    let mut worksheet = workbook.add_worksheet(Some(&sheet_name))?;
    
    // Auto-detect headers atau gunakan custom headers
    let headers = if let Some(custom_headers) = req.options.headers {
        custom_headers
    } else {
        auto_detect_headers(&req.data)
    };
    
    info!("📊 Detected {} columns: {:?}", headers.len(), headers);
    
    // Create header format
    let mut header_format = Format::new();
    header_format.set_bold();
    header_format.set_bg_color(FormatColor::Custom(0xE0E0E0));
    header_format.set_border(FormatBorder::Thin);
    
    // Write headers
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_string(0, col as u16, header, Some(&header_format))?;
    }
    
    // Set column widths
    for col in 0..headers.len() {
        worksheet.set_column(col as u16, col as u16, 15.0, None)?;
    }
    
    // Write data rows (optimized batch processing)
    info!("📝 Writing {} data rows...", req.data.len());
    
    // Process data in chunks for better memory management
    const CHUNK_SIZE: usize = 1000;
    let total_rows = req.data.len();
    
    for chunk_start in (0..total_rows).step_by(CHUNK_SIZE) {
        let chunk_end = std::cmp::min(chunk_start + CHUNK_SIZE, total_rows);
        let chunk = &req.data[chunk_start..chunk_end];
        
        // Pre-process chunk untuk type detection
        let processed_chunk: Vec<Vec<CellValue>> = chunk
            .iter()
            .map(|record| json_to_excel_row_optimized(record, &headers))
            .collect();
        
        // Write chunk ke Excel
        for (chunk_row_idx, excel_row) in processed_chunk.iter().enumerate() {
            let row_num = (chunk_start + chunk_row_idx + 1) as u32;
            
            for (col, cell_value) in excel_row.iter().enumerate() {
                let col_idx = col as u16;
                match cell_value {
                    CellValue::Empty => {
                        worksheet.write_blank(row_num, col_idx, None)?;
                    },
                    CellValue::String(s) => {
                        worksheet.write_string(row_num, col_idx, s, None)?;
                    },
                    CellValue::Integer(i) => {
                        worksheet.write_number(row_num, col_idx, *i as f64, None)?;
                    },
                    CellValue::Float(f) => {
                        worksheet.write_number(row_num, col_idx, *f, None)?;
                    },
                    CellValue::Bool(b) => {
                        worksheet.write_boolean(row_num, col_idx, *b, None)?;
                    },
                }
            }
        }
        
        // Log progress
        if chunk_end % 10000 == 0 || chunk_end == total_rows {
            info!("📈 Progress: {} / {} rows processed", chunk_end, total_rows);
        }
    }
    
    // Finalize workbook
    info!("💾 Finalizing workbook...");
    workbook.close()?;
    
    // Read file and return as bytes
    let excel_data = std::fs::read(&temp_file)?;
    
    // Clean up temp file
    let _ = std::fs::remove_file(&temp_file);
    
    info!("✅ Excel file generated, size: {} bytes", excel_data.len());
    Ok(excel_data)
}

// Auto-detect headers dari JSON pertama
fn auto_detect_headers(data: &[Value]) -> Vec<String> {
    if let Some(first_record) = data.first() {
        if let Value::Object(map) = first_record {
            let mut headers: Vec<String> = map.keys().cloned().collect();
            headers.sort(); // Sort untuk konsistensi
            return headers;
        }
    }
    vec!["data".to_string()] // Fallback
}

// Optimized: Convert JSON record ke Excel row dengan type detection
fn json_to_excel_row_optimized(record: &Value, headers: &[String]) -> Vec<CellValue> {
    headers.iter().map(|header| {
        match &record[header] {
            Value::Null => CellValue::Empty,
            Value::Bool(b) => CellValue::Bool(*b),
            Value::Number(n) => CellValue::String(n.to_string()),
            Value::String(s) => CellValue::String(s.clone()),
            Value::Array(_) => CellValue::String("[Array]".to_string()),
            Value::Object(_) => CellValue::String("[Object]".to_string()),
        }
    }).collect()
}

// Enum untuk optimized cell values
#[derive(Debug)]
enum CellValue {
    Empty,
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
}

// Health check endpoint
async fn health_handler() -> Result<impl warp::Reply, Infallible> {
    let response = HealthResponse {
        status: "healthy".to_string(),
        service: "excel-service".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    
    Ok(warp::reply::json(&response))
}

// Test endpoint untuk cek service
async fn test_handler() -> Result<impl warp::Reply, warp::Rejection> {
    info!("🧪 Test endpoint called");
    
    // Generate sample data with NIP
    let sample_data = vec![
        serde_json::json!({
            "id": 1,
            "name": "John Doe",
            "nip": "199103052019031008",
            "email": "john@example.com",
            "age": 30,
            "city": "Jakarta"
        }),
        serde_json::json!({
            "id": 2,
            "name": "Jane Smith", 
            "nip": "198712142020121005",
            "email": "jane@example.com",
            "age": 25,
            "city": "Surabaya"
        })
    ];
    
    let req = ExportRequest {
        data: sample_data,
        options: ExportOptions {
            filename: "test.xlsx".to_string(),
            sheet_name: Some("Test".to_string()),
            headers: None,
        },
    };
    
    generate_excel_handler(req).await
}

// Custom error types
#[derive(Debug)]
enum ExcelError {
    GenerationFailed(String),
}

impl warp::reject::Reject for ExcelError {}

// Error handler
async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = warp::http::StatusCode::NOT_FOUND;
        message = "Not Found";
    } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        code = warp::http::StatusCode::METHOD_NOT_ALLOWED;
        message = "Method Not Allowed";
    } else if let Some(e) = err.find::<ExcelError>() {
        match e {
            ExcelError::GenerationFailed(msg) => {
                code = warp::http::StatusCode::INTERNAL_SERVER_ERROR;
                message = msg;
            }
        }
    } else {
        error!("Unhandled rejection: {:?}", err);
        code = warp::http::StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error";
    }

    let json = warp::reply::json(&ApiResponse {
        success: false,
        message: message.to_string(),
        records_processed: None,
        processing_time_ms: None,
    });

    Ok(warp::reply::with_status(json, code))
}

// CORS filter
fn cors() -> warp::filters::cors::Builder {
    warp::cors()
        .allow_any_origin()
        .allow_headers(vec!["content-type", "authorization"])
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
}

#[tokio::main]
async fn main() {
    // Initialize logger
    env_logger::init();
    
    info!("🚀 Starting Excel Service v{}", env!("CARGO_PKG_VERSION"));
    
    // Health check route
    let health = warp::path("health")
        .and(warp::get())
        .and_then(health_handler);
    
    // Test route
    let test = warp::path("test")
        .and(warp::get())
        .and_then(test_handler);
    
    // Main Excel generation route
    let generate = warp::path("generate-excel")
        .and(warp::post())
        .and(warp::body::content_length_limit(1024 * 1024 * 500)) // 500MB limit for large datasets
        .and(warp::body::json())
        .and_then(generate_excel_handler);
    
    // Status endpoint
    let status = warp::path("status")
        .and(warp::get())
        .map(|| {
            let response = serde_json::json!({
                "service": "excel-service",
                "status": "running",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "memory_usage": get_memory_usage()
            });
            warp::reply::json(&response)
        });
    
    // Combine all routes
    let routes = health
        .or(test)
        .or(generate)
        .or(status)
        .with(cors())
        .recover(handle_rejection)
        .with(warp::log("excel-service"));
    
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3333".to_string())
        .parse::<u16>()
        .unwrap_or(3333);
    
    info!("🦀 Excel Service running on http://0.0.0.0:{}", port);
    info!("📋 Available endpoints:");
    info!("   GET  /health        - Health check");
    info!("   GET  /test          - Test with sample data");
    info!("   GET  /status        - Service status");
    info!("   POST /generate-excel - Generate Excel file");
    
    warp::serve(routes)
        .run(([0, 0, 0, 0], port))
        .await;
}

// Helper function untuk get memory usage
fn get_memory_usage() -> HashMap<String, String> {
    let mut usage = HashMap::new();
    
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    usage.insert("rss".to_string(), line.split_whitespace().nth(1).unwrap_or("0").to_string());
                }
                if line.starts_with("VmSize:") {
                    usage.insert("virtual".to_string(), line.split_whitespace().nth(1).unwrap_or("0").to_string());
                }
            }
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        usage.insert("platform".to_string(), "not_linux".to_string());
    }
    
    usage
}