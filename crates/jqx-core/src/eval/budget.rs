//! Resource budget enforcement: execution fuel and memory accounting.
//!
//! Guarantees queries cannot loop indefinitely or consume unbounded memory.

use crate::diag::{Diagnostic, Span};

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    remaining_fuel: u64,
    allocated_bytes: usize,
    max_memory_bytes: usize,
}

impl Budget {
    #[must_use]
    pub const fn new(fuel: u64, max_memory_bytes: usize) -> Self {
        Self {
            remaining_fuel: fuel,
            allocated_bytes: 0,
            max_memory_bytes,
        }
    }

    /// Deducts 1 step of fuel from the execution budget.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] `R0001` if the fuel budget is exhausted.
    pub fn consume_fuel(&mut self, span: Span) -> Result<(), Diagnostic> {
        if self.remaining_fuel == 0 {
            return Err(Diagnostic::error(
                "R0001",
                "execution fuel budget exhausted; query took too many steps",
                span,
            ));
        }
        self.remaining_fuel -= 1;
        Ok(())
    }

    /// Charges allocated bytes to the memory budget.
    ///
    /// # Errors
    ///
    /// Returns [`Diagnostic`] `R0002` if memory usage exceeds configured limits.
    pub fn charge_memory(&mut self, bytes: usize, span: Span) -> Result<(), Diagnostic> {
        self.allocated_bytes = self.allocated_bytes.saturating_add(bytes);
        if self.max_memory_bytes > 0 && self.allocated_bytes > self.max_memory_bytes {
            return Err(Diagnostic::error(
                "R0002",
                format!(
                    "memory limit exceeded: allocated {} bytes, max is {} bytes",
                    self.allocated_bytes, self.max_memory_bytes
                ),
                span,
            ));
        }
        Ok(())
    }
}

impl Default for Budget {
    fn default() -> Self {
        // Default: 1,000,000,000 steps fuel, 256 MiB RAM limit
        Self::new(1_000_000_000, 256 * 1024 * 1024)
    }
}
