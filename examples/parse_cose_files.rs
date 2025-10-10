use std::{env, fs};
use coset::{CoseSign1, TaggedCborSerializable};
use corim_rs::CorimMap;

/// Format a Unix timestamp (seconds since epoch) to a human-readable date string
fn format_timestamp(timestamp: i128) -> String {
    if timestamp < 0 {
        return format!("Invalid timestamp: {}", timestamp);
    }
    
    // Convert timestamp to date using proper calendar calculation
    let secs_since_epoch = timestamp as u64;
    let days_since_epoch = secs_since_epoch / 86400;  // 86400 seconds in a day
    
    // Calculate year, accounting for leap years properly
    let mut year = 1970;
    let mut remaining_days = days_since_epoch;
    
    // Count years and subtract days
    while remaining_days >= days_in_year(year) {
        remaining_days -= days_in_year(year);
        year += 1;
    }
    
    // Calculate month and day
    let (month, day) = day_of_year_to_month_day(remaining_days as u32 + 1, year);
    
    format!("{}-{:02}-{:02}T00:00:00Z", year, month, day)
}

/// Calculate number of days in a given year (handles leap years)
fn days_in_year(year: u64) -> u64 {
    if is_leap_year(year) { 366 } else { 365 }
}

/// Check if a year is a leap year
fn is_leap_year(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Convert day of year (1-366) to month and day
fn day_of_year_to_month_day(day_of_year: u32, year: u64) -> (u32, u32) {
    let days_in_month = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    
    let mut remaining_days = day_of_year;
    for (month_index, &days_in_this_month) in days_in_month.iter().enumerate() {
        if remaining_days <= days_in_this_month {
            return (month_index as u32 + 1, remaining_days);
        }
        remaining_days -= days_in_this_month;
    }
    
    // Fallback (shouldn't happen with valid input)
    (12, 31)
}

/// Try to parse a file as either a COSE-wrapped CoRIM or an unsigned CBOR CoRIM
fn parse_corim_file(file_bytes: &[u8]) -> Result<CorimMap, Box<dyn std::error::Error>> {
    // First try to parse as COSE (signed CoRIM)
    match CoseSign1::from_tagged_slice(file_bytes) {
        Ok(cose_sign1) => {
            if let Some(payload) = &cose_sign1.payload {
                println!("  ✓ Detected COSE-wrapped CoRIM (signed)");
                return Ok(ciborium::de::from_reader::<CorimMap, _>(payload.as_slice())?);
            } else {
                return Err("COSE structure missing payload".into());
            }
        }
        Err(_) => {
            // If COSE parsing fails, try as unsigned CBOR CoRIM
            println!("  ✓ Detected unsigned CBOR CoRIM");
            Ok(ciborium::de::from_reader::<CorimMap, _>(file_bytes)?)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("CoRIM COSE File Parser");
    println!("=====================");

    let args: Vec<String> = env::args().collect();
    
    // Require at least one COSE file as argument
    if args.len() < 2 {
        eprintln!("Usage: {} <cose_file1> [cose_file2] [...]", args[0]);
        eprintln!("Please specify one or more COSE files to parse.");
        eprintln!("Example: cargo run --example parse_cose_files -- file1.cose file2.cose");
        std::process::exit(1);
    }
    
    let cose_files = args[1..].to_vec();

    // Process each COSE file
    for (i, cose_file) in cose_files.iter().enumerate() {
        if i > 0 {
            println!(); // Add spacing between files
        }
        
        match fs::read(cose_file) {
            Ok(file_bytes) => {
                println!("✓ Processing {} ({} bytes)", cose_file, file_bytes.len());
                
                // Try to detect format and parse accordingly
                match parse_corim_file(&file_bytes) {
                    Ok(corim) => {
                        println!("  ✓ Successfully parsed CoRIM");
                        println!("  CoRIM ID: {:?}", corim.id);
                        
                        // Show validity information if present
                        if let Some(validity) = &corim.rim_validity {
                            println!("  Validity:");
                            if let Some(not_before) = &validity.not_before {
                                let timestamp = not_before.as_i128();
                                let datetime = format_timestamp(timestamp);
                                println!("    Not Before: {} ({})", timestamp, datetime);
                            }
                            let not_after_timestamp = validity.not_after.as_i128();
                            let not_after_datetime = format_timestamp(not_after_timestamp);
                            println!("    Not After: {} ({})", not_after_timestamp, not_after_datetime);
                        }
                        
                        println!("  Number of tags: {}", corim.tags.len());
                        
                        analyze_corim_structure(&corim);
                    }
                    Err(e) => {
                        println!("  ✗ Failed to parse file: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("❌ Failed to read {}: {}", cose_file, e);
            }
        }
    }
    
    Ok(())
}

fn analyze_corim_structure(corim: &corim_rs::CorimMap) {
    // Analyze each tag
    for (i, tag) in corim.tags.iter().enumerate() {
        match tag {
            corim_rs::ConciseTagTypeChoice::Mid(tagged_comid) => {
                let comid = tagged_comid.as_ref();
                println!("    Tag {}: CoMID", i);
                analyze_comid_structure(comid);
            }
            corim_rs::ConciseTagTypeChoice::Swid(tagged_swid) => {
                println!("    Tag {}: CoSWID", i);
                println!("      CoSWID tag: {:?}", tagged_swid.as_ref());
            }
            corim_rs::ConciseTagTypeChoice::Tl(tagged_tl) => {
                println!("    Tag {}: CoTL", i);  
                println!("      CoTL tag: {:?}", tagged_tl.as_ref());
            }
            corim_rs::ConciseTagTypeChoice::Extension(ext) => {
                println!("    Tag {}: Extension", i);
                
                // Try to interpret extension data as CoMID
                match ext {
                    corim_rs::ExtensionValue::Bytes(bytes) => {
                        let byte_vec: Vec<u8> = bytes.into();
                        println!("      Extension contains {} bytes - attempting to decode as CoMID...", byte_vec.len());
                        
                        // Try to deserialize the bytes as a CoMID
                        match ciborium::de::from_reader::<corim_rs::ConciseMidTag, _>(byte_vec.as_slice()) {
                            Ok(comid) => {
                                println!("      ✓ Successfully decoded extension as CoMID!");
                                analyze_comid_structure(&comid);
                            }
                            Err(e) => {
                                println!("      ✗ Failed to decode extension as CoMID: {:?}", e);
                                println!("      Raw extension value: {:?}", ext);
                            }
                        }
                    }
                    _ => {
                        println!("      Extension value (not bytes): {:?}", ext);
                    }
                }
            }
        }
    }
}

fn analyze_comid_structure(comid: &corim_rs::ConciseMidTag) {
    println!("      Tag Identity: {:?}", comid.tag_identity.tag_id);
    
    if let Some(version) = &comid.tag_identity.tag_version {
        println!("      Tag Version: {}", version);
    }
    
    // Check entities
    if let Some(entities) = &comid.entities {
        println!("      Entities: {} found", entities.len());
        for (j, entity) in entities.iter().enumerate() {
            println!("        Entity {}: {} (roles: {:?})", 
                j, entity.entity_name, entity.role);
        }
    }
    
    // Check triples for measurement values and TCB fields
    let triples = &comid.triples;
    
    if let Some(endorsed) = &triples.endorsed_triples {
        println!("      Endorsed Triples: {} found", endorsed.len());
        for (j, triple) in endorsed.iter().enumerate() {
            println!("        Triple {}: {} measurements", j, triple.endorsement.len());
            for (k, measurement) in triple.endorsement.iter().enumerate() {
                analyze_measurement_map(measurement, &format!("          Measurement {}", k));
            }
        }
    }

    if let Some(reference) = &triples.reference_triples {
        println!("      Reference Triples: {} found", reference.len());
        for (j, triple) in reference.iter().enumerate() {
            println!("        Triple {}: {} reference claims", j, triple.ref_claims.len());
            
            // Show environment information for this triple
            analyze_environment(&triple.ref_env, &format!("          Triple {} Environment", j));
            
            // Show measurement claims
            for (k, claim) in triple.ref_claims.iter().enumerate() {
                analyze_measurement_map(claim, &format!("          Reference Claim {}", k));
            }
        }
    }
    
    if let Some(conditional_series) = &triples.conditional_endorsement_series_triples {
        println!("      Conditional Endorsement Series: {} found", conditional_series.len());
        for (j, series) in conditional_series.iter().enumerate() {
            println!("        Series {}: {} condition claims, {} series records", 
                j, series.condition.claims_list.len(), series.series.len());
            
            // Show environment information for this series
            analyze_environment(&series.condition.environment, &format!("          Series {} Environment", j));
            
            // Analyze condition claims
            for (k, claim) in series.condition.claims_list.iter().enumerate() {
                analyze_measurement_map(claim, &format!("          Condition Claim {}", k));
            }
            
            // Analyze series records
            for (k, record) in series.series.iter().enumerate() {
                println!("          Series Record {}: {} selections, {} additions", 
                    k, record.selection.len(), record.addition.len());
                
                for (l, measurement) in record.selection.iter().enumerate() {
                    analyze_measurement_map(measurement, &format!("            Selection {}", l));
                }
                
                for (l, measurement) in record.addition.iter().enumerate() {
                    analyze_measurement_map(measurement, &format!("            Addition {}", l));
                }
            }
        }
    }
    
    if let Some(identity) = &triples.identity_triples {
        println!("      Identity Triples: {} found", identity.len());
        for (j, identity_record) in identity.iter().enumerate() {
            println!("        Identity {}: {} cryptographic keys", j, identity_record.key_list.len());
            
            // Show environment information for this identity
            analyze_environment(&identity_record.environment, &format!("          Identity {} Environment", j));
            
            // Show cryptographic keys
            for (k, key) in identity_record.key_list.iter().enumerate() {
                analyze_crypto_key(key, &format!("          Identity {} Key {}", j, k));
            }
            
            // Show conditions if present
            if let Some(_conditions) = &identity_record.conditions {
                println!("          Identity {} Conditions: Present", j);
                // Could expand this further if needed
            }
        }
    }
}

fn analyze_environment(env: &corim_rs::EnvironmentMap, prefix: &str) {
    let mut has_values = false;
    
    if let Some(class) = &env.class {
        if let Some(layer) = &class.layer {
            println!("{}: Environment Class Layer = {}", prefix, layer);
            has_values = true;
        }
        if let Some(vendor) = &class.vendor {
            println!("{}: Environment Vendor = {}", prefix, vendor);
            has_values = true;
        }
        if let Some(model) = &class.model {
            println!("{}: Environment Model = {}", prefix, model);
            has_values = true;
        }
        if let Some(class_id) = &class.class_id {
            println!("{}: Environment Class ID = {:?}", prefix, class_id);
            has_values = true;
        }
        if let Some(index) = &class.index {
            println!("{}: Environment Index = {}", prefix, index);
            has_values = true;
        }
    }
    if let Some(instance) = &env.instance {
        println!("{}: Environment Instance = {:?}", prefix, instance);
        has_values = true;
    }
    if let Some(group) = &env.group {
        println!("{}: Environment Group = {:?}", prefix, group);
        has_values = true;
    }
    
    if !has_values {
        println!("{}: (no environment values)", prefix);
    }
}

fn analyze_crypto_key(key: &corim_rs::CryptoKeyTypeChoice, prefix: &str) {
    match key {
        corim_rs::CryptoKeyTypeChoice::PkixBase64Key(_key) => {
            println!("{}: PKIX Base64 Key", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::PkixBase64Cert(_cert) => {
            println!("{}: PKIX Base64 Certificate", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::PkixBase64CertPath(_path) => {
            println!("{}: PKIX Base64 Certificate Path", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::CoseKey(_cose_key) => {
            println!("{}: COSE Key", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::Thumbprint(thumb) => {
            let digest = thumb.as_ref();
            let bytes_slice: &[u8] = digest.val.as_ref();
            let b64_val = simple_base64_encode(bytes_slice);
            println!("{}: Thumbprint = {:?}-{}", prefix, digest.alg, b64_val);
        }
        corim_rs::CryptoKeyTypeChoice::CertThumbprint(cert_thumb) => {
            let digest = cert_thumb.as_ref();
            let bytes_slice: &[u8] = digest.val.as_ref();
            let b64_val = simple_base64_encode(bytes_slice);
            println!("{}: Certificate Thumbprint = {:?}-{}", prefix, digest.alg, b64_val);
        }
        corim_rs::CryptoKeyTypeChoice::CertPathThumbprint(path_thumb) => {
            let digest = path_thumb.as_ref();
            let bytes_slice: &[u8] = digest.val.as_ref();
            let b64_val = simple_base64_encode(bytes_slice);
            println!("{}: Certificate Path Thumbprint = {:?}-{}", prefix, digest.alg, b64_val);
        }
        corim_rs::CryptoKeyTypeChoice::PkixAsn1DerCert(_der_cert) => {
            println!("{}: PKIX ASN.1 DER Certificate", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::Bytes(_bytes) => {
            println!("{}: Raw Bytes", prefix);
        }
        corim_rs::CryptoKeyTypeChoice::Extension(ext) => {
            println!("{}: Extension = {:?}", prefix, ext);
        }
    }
}

fn simple_base64_encode(input: &[u8]) -> String {
    const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in input.chunks(3) {
        let mut buf = [0u8; 3];
        for (i, &byte) in chunk.iter().enumerate() {
            buf[i] = byte;
        }
        
        let b = ((buf[0] as u32) << 16) | ((buf[1] as u32) << 8) | (buf[2] as u32);
        
        result.push(BASE64_CHARS[((b >> 18) & 0x3f) as usize] as char);
        result.push(BASE64_CHARS[((b >> 12) & 0x3f) as usize] as char);
        result.push(if chunk.len() > 1 { BASE64_CHARS[((b >> 6) & 0x3f) as usize] as char } else { '=' });
        result.push(if chunk.len() > 2 { BASE64_CHARS[(b & 0x3f) as usize] as char } else { '=' });
    }
    
    result
}

fn analyze_measurement_map(measurement: &corim_rs::MeasurementMap, prefix: &str) {
    if let Some(mkey) = &measurement.mkey {
        println!("{}: Measurement Key = {:?}", prefix, mkey);
    }
    analyze_measurement_values(&measurement.mval, prefix);
}

fn analyze_measurement_values(mval: &corim_rs::MeasurementValuesMap, prefix: &str) {
    let mut has_values = false;
    
    if let Some(version) = &mval.version {
        println!("{}: Version = {} (scheme: {:?})", prefix, version.version, version.version_scheme);
        has_values = true;
    }
    
    if let Some(svn) = &mval.svn {
        println!("{}: SVN = {:?}", prefix, svn);
        has_values = true;
    }
    
    if let Some(digests) = &mval.digests {
        println!("{}: Digests = {} entries", prefix, digests.len());
        for (i, digest) in digests.iter().enumerate() {
            // Convert bytes to base64 for readability  
            let bytes_slice: &[u8] = digest.val.as_ref();
            let b64_val = simple_base64_encode(bytes_slice);
            println!("{}:   Digest {}: {:?}-{}", prefix, i, digest.alg, b64_val);
        }
        has_values = true;
    }
    
    // Check for TCB fields (our custom additions)
    if let Some(tcb_status) = &mval.tcb_status {
        println!("{}: 🔧 TCB Status = {:?}", prefix, tcb_status);
        has_values = true;
    }
    
    if let Some(tcb_date) = &mval.tcb_date {
        println!("{}: 🔧 TCB Date = {:?}", prefix, tcb_date);
        has_values = true;
    }
    
    if let Some(tcb_details) = &mval.tcb_status_details {
        println!("{}: 🔧 TCB Status Details = {:?}", prefix, tcb_details);
        has_values = true;
    }
    
    if let Some(name) = &mval.name {
        println!("{}: Name = {:?}", prefix, name);
        has_values = true;
    }
    
    if let Some(serial) = &mval.serial_number {
        println!("{}: Serial Number = {:?}", prefix, serial);
        has_values = true;
    }
    
    if !has_values {
        println!("{}: (no measurement values)", prefix);
    }
}