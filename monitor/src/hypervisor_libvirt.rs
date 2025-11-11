use jane_eyre::eyre;

pub fn list_runner_guests() -> eyre::Result<Vec<String>> {
    crate::libvirt::list_runner_guests()
}
