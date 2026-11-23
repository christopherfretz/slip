#include "side.h"
// WP-124 item 4 (Kiln WP-3027 "Reachable"): changed in a template nothing instantiates; the AST holds it. The default is on the first declaration.
#ifdef B
template<class T = long> void f();
#else
template<class T = int> void f();
#endif
template<class T> void f() {}
