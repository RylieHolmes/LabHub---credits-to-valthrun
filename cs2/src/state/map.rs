use cs2_schema_cutl::{CStringUtil, PtrCStr};
use raw_struct::{
    builtins::Ptr64,
    FromMemoryView,
};
use utils_state::{
    State,
    StateCacheType,
    StateRegistry,
};

use crate::{
    schema::CNetworkGameClient,
    CS2Offset,
    StateCS2Memory,
    StateResolvedOffset,
    StateGlobals,
};

pub struct StateCurrentMap {
    pub current_map: Option<String>,
}

impl State for StateCurrentMap {
    type Parameter = ();

    fn create(states: &StateRegistry, _param: Self::Parameter) -> anyhow::Result<Self> {
        let memory_view = states.resolve::<StateCS2Memory>(())?;
        
        // Priority 1: GlobalVars (Fast & Reliable)
        if let Ok(globals) = states.resolve::<StateGlobals>(()) {
            if let Some(map) = globals.current_map_ptr().ok().and_then(|v| v.read_string(memory_view.view()).ok().flatten()) {
                 if map.len() > 2 {
                     let clean = map.replace("maps/", "").replace(".vpk", "");
                     return Ok(Self { current_map: Some(clean) });
                 }
            }
            if let Some(map) = globals.current_map_name_ptr().ok().and_then(|v| v.read_string(memory_view.view()).ok().flatten()) {
                 if map.len() > 2 {
                     let clean = map.replace("maps/", "").replace(".vpk", "");
                     return Ok(Self { current_map: Some(clean) });
                 }
            }
        }

        // Priority 2: NetworkGameClient standard read
        if let Ok(offset_client) = states.resolve::<StateResolvedOffset>(CS2Offset::NetworkGameClientInstance) {
             if let Ok(instance_ptr) = Ptr64::<dyn CNetworkGameClient>::read_object(memory_view.view(), offset_client.address) {
                 if let Some(instance) = instance_ptr.value_reference(memory_view.view_arc()) {
                     if let Some(map) = instance.map_name().ok().and_then(|v| v.read_string(memory_view.view()).ok().flatten()) {
                         if map.len() > 2 {
                              return Ok(Self { current_map: Some(map) });
                         }
                     }
                 }
             }
        }

        Ok(Self { current_map: None })
    }

    fn cache_type() -> StateCacheType {
        StateCacheType::Volatile
    }
}
