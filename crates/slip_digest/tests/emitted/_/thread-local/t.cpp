#include "side.h"
// WP-155: a thread-local whose initialiser is not a constant is marked as
// a static one is (conservative: its initialisation is on first use in a
// thread, which clang emits as a wrapper whether or not it is used).
int g();
thread_local int t = g();
thread_local constexpr int u = 1;
