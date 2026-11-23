// std: c++26
#include "side.h"
// digest §4.3: a `consteval` block is the call of a lambda (P3289R1) whose
// closure the block owns; the block is keyed by its content, in which the
// closure's key is under the block itself (`local.rs` `SELF_KEY`).
#ifdef B
consteval { int a = 2; (void)a; }
#else
consteval { int a = 1; (void)a; }
#endif
