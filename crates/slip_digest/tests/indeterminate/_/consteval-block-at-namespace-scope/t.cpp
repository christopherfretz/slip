#include "side.h"
// std: c++26
// digest §4.3: a `consteval` block is the call of a lambda (P3289R1;
// Kiln `kiln_sema/src/consteval_block.rs:1-6`) whose closure Kiln gives the
// namespace as owner, and a key keyed by content cannot hold the key of a
// type its own stream declares: `Unsupported("local/closure-owner")`.
consteval {}
