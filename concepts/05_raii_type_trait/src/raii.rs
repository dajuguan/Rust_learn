use tokio::sync::{Semaphore, SemaphorePermit};

struct Museum {
    remaining_tickets: Semaphore
}

struct Ticket<'a> {
    permit: SemaphorePermit<'a>
}

impl<'a> Drop for Ticket<'a>{
    fn drop(&mut self) {
        println!("Ticket dropped");
    }
}

impl Museum {
    fn new(total: usize) -> Museum {
        Museum {
            remaining_tickets: Semaphore::new(total)
        }
    }

    fn aquire(&self) -> Option<Ticket<'_>> {
        let permit = self.remaining_tickets.try_acquire();
        match permit {
            Ok(permit) => Some(Ticket { permit }),
            _ => None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let m = Museum::new(5);
        let ticket = m.aquire().unwrap();
        let other_tickets = (0..4).map(|_| m.aquire().unwrap()).collect::<Vec<_>>();
        assert!(m.remaining_tickets.available_permits() == 0);
        drop(ticket);
        assert!(m.remaining_tickets.available_permits() == 1);
        {
            let ticket = m.aquire().unwrap();
            assert!(m.remaining_tickets.available_permits() == 0);
        }
    }
}