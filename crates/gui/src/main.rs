#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod basemap_providers;
mod commands;
mod logging;
mod meta;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Logging starts here rather than at the top of `main` because
            // the log directory is a question only the app handle can
            // answer. The little that happens before this point does not
            // log; the guard keeps the file writer's worker alive and is
            // handed to the app so it lives as long as the process.
            use tauri::Manager;
            let log_dir = app.path().app_log_dir().ok();
            let (guard, dir) = logging::init(log_dir);
            app.manage(commands::LogLocation(dir));
            if let Some(guard) = guard {
                app.manage(guard);
            }

            // Bring the main window forward on every start.
            //
            // After a Windows update the app is relaunched by the NSIS
            // installer (`/R`), not by the user, and it came back minimized —
            // the update looked like it had closed the app. Nothing else
            // guarantees the window is visible either, so this is
            // unconditional rather than gated on the platform or on having
            // just updated.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::NetworkState::default())
        .manage(commands::RunQueue::default())
        .manage(basemap_providers::ProvidersState::default())
        // Asynchronous variant: the handler hands the request to a pool
        // worker and returns immediately. The proxy does blocking network
        // I/O (up to the 10 s upstream timeout), which must never run on
        // the thread the webview invokes scheme handlers on. The pool is
        // bounded — see `basemap_providers::pool` for why a thread per tile
        // is not an option.
        .register_asynchronous_uri_scheme_protocol("basemap", |ctx, request, responder| {
            use tauri::Manager;
            let state = ctx
                .app_handle()
                .state::<basemap_providers::ProvidersState>()
                .inner()
                .clone();
            // The responder moves into the job, so the overload answer has to
            // be prepared before submitting: recover it via the cell if the
            // job was refused.
            let responder = std::sync::Arc::new(std::sync::Mutex::new(Some(responder)));
            let job_responder = responder.clone();
            let submitted = basemap_providers::pool::try_submit(move || {
                let taken = job_responder.lock().ok().and_then(|mut r| r.take());
                if let Some(responder) = taken {
                    responder.respond(basemap_providers::proxy::handle(&state, &request));
                }
            });
            if !submitted {
                let taken = responder.lock().ok().and_then(|mut r| r.take());
                if let Some(responder) = taken {
                    responder.respond(basemap_providers::pool::overloaded_response());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::create_project,
            commands::delete_project,
            commands::delete_simulation,
            commands::delete_all_simulations,
            commands::project_results_sizes,
            commands::projects_results_size,
            commands::get_project_criteria,
            commands::update_project_criteria,
            commands::get_criteria_valuation,
            commands::update_criteria_valuation,
            commands::get_criteria_catalog,
            commands::rename_project,
            commands::update_project_crs,
            commands::update_project_units,
            commands::get_model_unit_system,
            commands::list_crs_catalog_page,
            commands::list_custom_crs,
            commands::upsert_custom_crs,
            commands::delete_custom_crs,
            commands::save_project,
            commands::list_scenarios,
            commands::create_scenario,
            commands::delete_scenario,
            commands::rename_scenario,
            commands::open_base_folder,
            commands::open_data_folder,
            commands::open_scenario_folder,
            commands::open_and_load_network,
            commands::open_and_scan_archive,
            commands::attach_aux_file,
            commands::create_projects_from_archive,
            commands::open_and_recognise_network,
            commands::get_network_snapshot,
            commands::get_network_title,
            commands::get_run_warnings,
            commands::load_result_meta,
            commands::get_network_digest,
            commands::get_project_sketch,
            commands::get_period_results,
            commands::get_element_details,
            commands::get_inlet_couplings,
            commands::get_kind_elements,
            commands::get_kind_counts,
            commands::get_collection_detail,
            commands::get_pump_energy,
            commands::load_project_network,
            commands::patch_elements,
            commands::patch_node_position,
            commands::delete_element,
            commands::rename_element,
            commands::create_node,
            commands::create_link,
            commands::create_element,
            commands::set_element_ends,
            commands::set_collection_contents,
            commands::get_element_records,
            commands::set_element_records,
            commands::get_versions,
            commands::get_license_info,
            commands::get_data_usage,
            commands::open_log_folder,
            commands::clear_all_results,
            commands::list_third_party_components,
            commands::get_third_party_license_text,
            commands::list_engines,
            commands::list_element_kinds,
            commands::list_element_attributes,
            commands::set_element_attribute,
            commands::updater_supported,
            commands::get_run_queue,
            commands::enqueue_runs,
            commands::cancel_run_queue,
            commands::cancel_run_item,
            commands::get_sim_params,
            commands::get_sim_summary_pairs,
            commands::get_analysis_blocks,
            commands::update_sim_params,
            commands::get_element_series,
            commands::validate_network,
            commands::update_network_title,
            commands::export_project_inp,
            commands::export_results_csv,
            commands::list_report_blocks,
            commands::get_report_block_options,
            commands::probe_report_blocks,
            commands::get_report_template,
            commands::save_report_template,
            commands::generate_report,
            commands::export_report,
            commands::list_basemap_providers,
            commands::connect_basemap_provider,
            commands::disconnect_basemap_provider,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
