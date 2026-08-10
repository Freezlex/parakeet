use domain::NetStatus;

pub trait ConnectivityMonitor: Send + Sync {
    fn status(&self) -> NetStatus;
    fn observe(&self, status: NetStatus);
}
