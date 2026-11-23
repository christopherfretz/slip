#include "side.h"
// WP-124 item 4: changed in a template nothing instantiates; the AST holds it.
#ifdef B
#define X 2
#else
#define X 1
#endif
template<class T> void f(T, int = X);
