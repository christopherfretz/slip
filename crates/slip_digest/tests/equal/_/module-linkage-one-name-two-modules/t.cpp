// module: first=first.cppm
// module: second=second.cppm
// digest §3, WP-048 step 6: `g` of `first` and `g` of `second` have module linkage and are two entities ([basic.link]/8). A key holds no module (keys.rs), so they share one: the key sweep (determinism.rs `keys_and_names_are_unique`) is expected to find the collision when this runs.
import first;
import second;
int y = h();
