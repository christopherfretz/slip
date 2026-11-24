#include "side.h"
// A constructor of an explicit specialisation: no template head is open, the list is on the AST (WP-124).
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> struct S { int m; S(); };
template<> struct S<int> { int m; S() : m(X) { } };
