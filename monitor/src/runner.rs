use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    time::SystemTime,
};

use jane_eyre::eyre;
use log::{trace, warn};

use crate::{data::get_runner_data_path, github::ApiRunner};

pub struct Runners {
    runners: BTreeMap<usize, Runner>,
}

pub struct Runner {
    created_time: SystemTime,
    registration: Option<ApiRunner>,
    guest_name: Option<String>,
    volume_name: Option<String>,
}

impl Runners {
    pub fn new(
        registrations: Vec<ApiRunner>,
        guest_names: Vec<String>,
        volume_names: Vec<String>,
    ) -> Self {
        // Gather all known runner ids with live resources.
        let registration_ids = registrations
            .iter()
            .flat_map(|registration| registration.name.rsplit_once('@'))
            .flat_map(|(name, _host)| name.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let guest_ids = guest_names
            .iter()
            .flat_map(|guest| guest.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let volume_ids = volume_names
            .iter()
            .flat_map(|volume| volume.rsplit_once('.'))
            .flat_map(|(_, id)| id.parse())
            .collect::<Vec<usize>>();
        let ids: BTreeSet<usize> = registration_ids
            .iter()
            .copied()
            .chain(guest_ids.iter().copied())
            .chain(volume_ids.iter().copied())
            .collect();
        trace!("ids = {ids:?}, registration_ids = {registration_ids:?}, guest_ids = {guest_ids:?}, volume_ids = {volume_ids:?}");

        // Create a tracking object for each runner id.
        let mut runners = BTreeMap::default();
        for id in ids {
            let runner = match Runner::new(id) {
                Ok(runner) => runner,
                Err(error) => {
                    warn!("Failed to create Runner object for runner id {id}: {error}");
                    continue;
                }
            };
            runners.insert(id, runner);
        }

        // Populate the tracking objects with references to live resources.
        for (id, registration) in registration_ids.iter().zip(registrations) {
            if let Some(runner) = runners.get_mut(id) {
                runner.registration = Some(registration);
            }
        }
        for (id, guest_name) in guest_ids.iter().zip(guest_names) {
            if let Some(runner) = runners.get_mut(id) {
                runner.guest_name = Some(guest_name);
            }
        }
        for (id, volume_name) in volume_ids.iter().zip(volume_names) {
            if let Some(runner) = runners.get_mut(id) {
                runner.volume_name = Some(volume_name);
            }
        }

        Self { runners }
    }
}

impl Runner {
    /// Creates an object for tracking the state of a runner.
    ///
    /// For use by [`Runners::new`] only. Does not create a runner.
    fn new(id: usize) -> eyre::Result<Self> {
        let created_time = get_runner_data_path(id, "created-time")?;
        let created_time = fs::metadata(created_time)?.modified()?;
        trace!("[{id}] created_time = {created_time:?}");

        Ok(Self {
            created_time,
            registration: None,
            guest_name: None,
            volume_name: None,
        })
    }
}
