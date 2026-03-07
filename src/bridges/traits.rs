/// Bridge traits for inter-sister communication.
/// All methods have NoOp defaults for standalone operation.

pub trait MemoryBridge {
    fn store_context(&self, _key: &str, _value: &str) -> Result<(), String> {
        Ok(())
    }
    fn recall_context(&self, _key: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

pub trait VisionBridge {
    fn capture_state(&self, _id: &str) -> Result<String, String> {
        Ok(String::new())
    }
}

pub trait IdentityBridge {
    fn verify_identity(&self, _agent_id: &str) -> Result<bool, String> {
        Ok(true)
    }
}

pub trait TimeBridge {
    fn check_deadline(&self, _id: &str) -> Result<bool, String> {
        Ok(true)
    }
}

pub trait ContractBridge {
    fn check_policy(&self, _action: &str) -> Result<bool, String> {
        Ok(true)
    }
}

pub trait CommBridge {
    fn broadcast(&self, _event: &str, _payload: &str) -> Result<(), String> {
        Ok(())
    }
}

pub trait CodebaseBridge {
    fn get_context(&self, _path: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

pub trait PlanningBridge {
    fn register_constraint(&self, _constraint: &str) -> Result<(), String> {
        Ok(())
    }
}

pub trait CognitionBridge {
    fn assess_quality(&self, _input: &str) -> Result<f64, String> {
        Ok(1.0)
    }
}

pub trait RealityBridge {
    fn check_resources(&self) -> Result<bool, String> {
        Ok(true)
    }
}

pub trait HydraAdapter {
    fn register_with_hydra(&self) -> Result<(), String> {
        Ok(())
    }
}
