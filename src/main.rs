mod model;

fn main() {
    println!("OMP Pet: {}", model::Snapshot::demo().activity.label());
}
