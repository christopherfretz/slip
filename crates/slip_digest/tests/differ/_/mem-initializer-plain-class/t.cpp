#include "side.h"
// A constructor of a class that is not a template: its list is on the AST and read (WP-124).
#ifdef B
#define X 2
#else
#define X 1
#endif
struct S { int m; S() : m(X) { } };
