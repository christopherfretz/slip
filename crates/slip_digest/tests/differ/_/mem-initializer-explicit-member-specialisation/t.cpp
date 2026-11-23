#include "side.h"
// An explicitly specialised constructor (`template<>`): the list is on the AST (WP-124).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S(); };
template<> S<int>::S() : m(X) { }
