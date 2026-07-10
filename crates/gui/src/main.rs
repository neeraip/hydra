#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod meta;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(commands::NetworkState::default())
        .manage(commands::RunQueue::default())
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
            commands::get_nodes,
            commands::get_links,
            commands::get_patterns,
            commands::get_curves,
            commands::run_simulation,
            commands::load_result_meta,
            commands::get_period_results,
            commands::get_pump_energy,
            commands::get_result_analytics,
            commands::get_violations,
            commands::load_project_network,
            commands::patch_element,
            commands::get_project_inp,
            commands::patch_node_position,
            commands::delete_element,
            commands::create_node,
            commands::create_link,
            commands::create_curve,
            commands::create_pattern,
            commands::preview_patches,
            commands::get_versions,
            commands::reconcile_projects,
            commands::get_run_queue,
            commands::enqueue_runs,
            commands::cancel_run_queue,
            commands::cancel_run_item,
            commands::get_sim_params,
            commands::update_sim_params,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
