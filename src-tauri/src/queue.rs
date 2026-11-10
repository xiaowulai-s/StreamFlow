//! 任务队列 - 速率限制

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;

#[derive(Debug, Clone)]
pub struct TaskQueue {
    rate_limiter: Arc<RateLimiter>,
}

impl TaskQueue {
    pub fn new(max_concurrent: usize, min_interval_ms: u64) -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new(max_concurrent, min_interval_ms)),
        }
    }
    
    pub async fn spawn<T: Send + 'static>(
        &self,
        _task: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let _permit = self.rate_limiter.acquire().await;
        _task()
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    min_interval: std::time::Duration,
    last_request: std::sync::Mutex<std::time::Instant>,
    request_count: AtomicU64,
}

impl RateLimiter {
    pub fn new(max_concurrent: usize, min_interval_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            min_interval: std::time::Duration::from_millis(min_interval_ms),
            last_request: std::sync::Mutex::new(std::time::Instant::now()),
            request_count: AtomicU64::new(0),
        }
    }
    
    pub async fn acquire(&self) -> tokio::sync::OwnedSemaphorePermit {
        let permit = self.semaphore.clone().acquire_owned().await.ok().unwrap();
        
        let mut last = self.last_request.lock().unwrap();
        let elapsed = last.elapsed();
        
        if elapsed < self.min_interval {
            let sleep_duration = self.min_interval - elapsed;
            drop(permit);
            tokio::time::sleep(sleep_duration).await;
            
            // 循环处理避免递归
            let mut attempts = 0;
            loop {
                let new_permit = self.semaphore.clone().acquire_owned().await.ok();
                match new_permit {
                    Some(p) => {
                        *last = std::time::Instant::now();
                        return p;
                    }
                    None => {
                        tokio::time::sleep(sleep_duration).await;
                        attempts += 1;
                        if attempts > 100 {
                            panic!("Rate limiter failed after too many attempts");
                        }
                    }
                }
            }
        }
        
        *last = std::time::Instant::now();
        permit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(2, 100);
        let _permit1 = limiter.acquire().await;
        let _permit2 = limiter.acquire().await;
    }
}
