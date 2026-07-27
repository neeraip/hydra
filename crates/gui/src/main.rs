#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The offline-basemap subsystem lands ahead of its consumers: the tile
// store and `basemap://` protocol are live below, while the download and
// coverage command surface arrives with the next milestones. Until those
// commands exist, several of the module's exports are unused.
#[allow(dead_code, unused_imports)]
mod basemap;
mod commands;
mod meta;

fn main() {
    // Log to stderr; default level `warn` unless RUST_LOG overrides it.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::NetworkState::default())
        .manage(commands::RunQueue::default())
        .setup(|app| {
            use tauri::Manager;
            let db_path = commands::app_data_dir(app.handle())
                .map(|dir| dir.join("basemaps.db"))
                .map_err(std::io::Error::other)?;
            app.manage(basemap::BasemapState::new(db_path));
            Ok(())
        })
        .register_uri_scheme_protocol("basemap", |ctx, request| {
            use tauri::Manager;
            let state = ctx.app_handle().try_state::<basemap::BasemapState>();
            basemap::protocol_response(state.as_deref(), &request)
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::create_project,
            commands::load_project,
            commands::delete_project,
            commands::rename_project,
            commands::update_project_crs,
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
            commands::open_scenario_folder,
            commands::open_and_load_network,
            commands::pick_csv_file,
            commands::get_network_snapshot,
            commands::get_nodes,
            commands::get_links,
            commands::get_patterns,
            commands::get_curves,
            commands::get_network_title,
            commands::get_controls,
            commands::get_rules,
            commands::run_simulation,
            commands::get_run_warnings,
            commands::load_result_meta,
            commands::get_network_digest,
            commands::get_period_results,
            commands::get_pump_energy,
            commands::get_result_analytics,
            commands::load_project_network,
            commands::patch_element,
            commands::patch_elements,
            commands::get_project_inp,
            commands::patch_node_position,
            commands::delete_element,
            commands::rename_element,
            commands::create_node,
            commands::create_link,
            commands::create_curve,
            commands::update_curve_points,
            commands::delete_curve,
            commands::rename_curve,
            commands::create_pattern,
            commands::update_pattern_multipliers,
            commands::rename_pattern,
            commands::delete_pattern,
            commands::create_control,
            commands::update_control,
            commands::delete_control,
            commands::create_rule,
            commands::update_rule,
            commands::delete_rule,
            commands::preview_patches,
            commands::get_versions,
            commands::reconcile_projects,
            commands::get_run_queue,
            commands::enqueue_runs,
            commands::cancel_run_queue,
            commands::cancel_run_item,
            commands::get_sim_params,
            commands::update_sim_params,
            commands::get_element_series,
            commands::validate_network,
            commands::update_network_title,
            commands::export_project_inp,
            commands::export_results_csv,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
