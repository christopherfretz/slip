#include "side.h"
// WP-124 item 4 (Kiln WP-3027 A6): changed in a template nothing instantiates; the AST holds it.
template<class T, class U> struct P;
template<class T> struct P<T, int*>;
#ifdef B
template<class Z> struct P<Z, int*> { Z* m; };
#else
template<class T> struct P<T, int*> { T m; };
#endif
