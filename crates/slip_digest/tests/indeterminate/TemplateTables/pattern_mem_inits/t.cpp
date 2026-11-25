#include "side.h"
// §9.5 TemplateTables `pattern_mem_inits` (unsupported, WP-124 item 5; Kiln WP-3026, WP-3200): a constructor definition in a template, with a mem-initializer. Cause kiln/pattern-mem-initializers.
template<class T> struct S { int m; S() : m(1) {} };
