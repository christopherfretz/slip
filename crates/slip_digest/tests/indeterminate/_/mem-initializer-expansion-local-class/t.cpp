#include "side.h"
// std: c++26
// WP-124: a local class in an expansion statement of a function that is not a template: the statement's pattern is in h()'s unit and its level is a template. Cause kiln/pattern-mem-initializers. Pending: under a rule table the two instantiated local classes collide (<order>), with or without WP-124 (tests/rule_use.rs pair_tests_on_and_off).
#ifdef B
#define X 2
#else
#define X 1
#endif
void h() { template for (constexpr int i : {1, 2}) { struct L { int m; L() : m(X + i) { } }; L l; } }
