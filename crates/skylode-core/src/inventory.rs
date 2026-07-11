use crate::material::Material;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    materials: HashMap<Material, u32>,
}

impl Inventory {
    /// Creates a new, empty inventory.
    pub fn new() -> Self {
        Inventory {
            materials: HashMap::new(),
        }
    }

    /// Adds the given amount of the specified material to the inventory.
    pub fn add_material(&mut self, material: Material, amount: u32) {
        let entry = self.materials.entry(material).or_insert(0);
        *entry += amount;
    }

    /// Removes the given amount of the specified material from the inventory.
    /// If the inventory has less than `amount`, it removes the entry
    pub fn remove_material(&mut self, material: Material, amount: u32) {
        let entry = self.materials.entry(material).or_insert(0);
        if *entry <= amount {
            self.materials.remove(&material);
        } else {
            *entry -= amount;
        }
    }

    /// Returns the count of the specified material in the inventory.
    pub fn get_material_count(&self, material: Material) -> u32 {
        *self.materials.get(&material).unwrap_or(&0)
    }

    /// Checks if the inventory has at least the specified amount of the given material.
    pub fn has_material_count(&self, material: Material, amount: u32) -> bool {
        self.get_material_count(material) >= amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inventory_add_and_remove() {
        let mut inventory = Inventory::new();
        inventory.add_material(Material::Stone, 10);
        assert_eq!(inventory.get_material_count(Material::Stone), 10);
        inventory.remove_material(Material::Stone, 5);
        assert_eq!(inventory.get_material_count(Material::Stone), 5);
    }

    #[test]
    fn test_inventory_remove_more_than_exists() {
        let mut inventory = Inventory::new();
        inventory.add_material(Material::Stone, 10);
        inventory.remove_material(Material::Stone, 15);
        assert_eq!(inventory.get_material_count(Material::Stone), 0);
    }

    #[test]
    fn test_inventory_has_material_count() {
        let mut inventory = Inventory::new();
        inventory.add_material(Material::Stone, 10);
        assert!(inventory.has_material_count(Material::Stone, 5));
        assert!(!inventory.has_material_count(Material::Stone, 15));
    }
}
