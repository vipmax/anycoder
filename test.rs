use std::collections::HashMap;

#[tokio::main]
fn main() {
    println!("hello");
    
    let fruits = vec!["apple", "banana", "orange"];
    for fruit in fruits {
        println!("{}", fruit);
    }
    let mut vegetables = HashMap::new();
    vegetables.insert("carrot", "orange");
    vegetables.insert("broccoli", "green");

    for (veg, color) in &vegetables {
        println!("{} is {}", veg, color);
    }
}
