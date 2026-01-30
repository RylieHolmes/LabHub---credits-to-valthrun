use std::collections::BTreeMap;

use anyhow::{
    anyhow,
    Context,
};
use cs2_schema_cutl::{
    CStringUtil,
    PtrCStr,
};
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
    CEntityIdentityEx,
    CS2Handle,
    StateCS2Handle,
    StateEntityList,
};

pub struct ClassNameCache {
    lookup: BTreeMap<u64, String>,
    reverse_lookup: BTreeMap<String, u64>,
}

impl State for ClassNameCache {
    type Parameter = ();

    fn create(_states: &StateRegistry, _param: Self::Parameter) -> anyhow::Result<Self> {
        Ok(Self {
            lookup: Default::default(),
            reverse_lookup: Default::default(),
        })
    }

    fn cache_type() -> StateCacheType {
        StateCacheType::Persistent
    }

    fn update(&mut self, states: &StateRegistry) -> anyhow::Result<()> {
        let cs2 = states.resolve::<StateCS2Handle>(())?;
        let entities = states.resolve::<StateEntityList>(())?;
        for identity in entities.entities() {
            let class_info = identity.entity_class_info()?;
            self.register_class_info(&cs2, identity.entity_class_info()?)
                .with_context(|| {
                    format!(
                        "failed to generate class info for entity {:?} (class info = {:X})",
                        identity.handle::<()>().unwrap_or_default(),
                        class_info.address
                    )
                })?;
        }
        Ok(())
    }
}

impl ClassNameCache {
    fn register_class_info(
        &mut self,
        cs2: &CS2Handle,
        class_info: Ptr64<()>,
    ) -> anyhow::Result<()> {
        let address = class_info.address;
        if self.lookup.contains_key(&address) {
            /* we already know the name for this class */
            return Ok(());
        }

        let memory = cs2.create_memory_view();

        // Safety wrapper: Try to read the pointer and string. If ANYTHING fails, just skip this entity.
        // Do not propagate errors as it causes the main loop to stall.
        let result: anyhow::Result<String> = (|| {
            // Upstream Logic:
            // let class_name = PtrCStr::read_object(
            //     &*memory,
            //     u64::read_object(&*memory, address + 0x08).map_err(|e| anyhow!(e))? + 0x00,
            // )
            
            let designer_name_ptr_ptr = address + 0x08;
            let designer_name_ptr = u64::read_object(&*memory, designer_name_ptr_ptr).map_err(|e| anyhow!(e))?;

            // Sanity check pointer (userland pointers are usually 0x0000 - 0x7FFF...)
            // 0xCCCCCCCCCCCCCCCC is definitely invalid.
            if designer_name_ptr < 0x1000 || designer_name_ptr > 0x7FFFFFFFFFFF {
                return Err(anyhow!("Invalid designer_name_ptr: {:X}", designer_name_ptr));
            }

            let class_name = PtrCStr::read_object(
                &*memory,
                designer_name_ptr + 0x00, // Upstream uses + 0x00
            )
            .map_err(|e| anyhow!(e))?
            .read_string(&*memory)
            .map_err(|e| anyhow!(e))?
            .context("failed to read class name string")?;
            
            Ok(class_name)
        })();

        match result {
            Ok(class_name) => {
                // println!("ClassNameCache: Success {:X} -> {}", address, class_name);
                self.lookup.insert(address, class_name.clone());
                self.reverse_lookup.insert(class_name, address);
            }
            Err(_e) => {
                // Squelch the error. We can optionally log once per address if needed, but for now just ignore.
                // Returning Ok(()) keeps the controller running effectively.
                // println!("ClassNameCache: Failed {:X}: {}", address, _e);
                return Ok(()); 
            }
        }

        Ok(())
    }

    pub fn lookup(&self, class_info: &Ptr64<()>) -> anyhow::Result<Option<&String>> {
        let address = class_info.address;
        Ok(self.lookup.get(&address))
    }

    pub fn reverse_lookup(&self, name: &str) -> Option<u64> {
        self.reverse_lookup.get(name).cloned()
    }
}
