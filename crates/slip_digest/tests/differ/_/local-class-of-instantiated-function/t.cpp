#include "side.h"
// §11.9, §2.4 (landed WP-027): a local class of an instantiated function is
// not of the definition class; its default member initialiser is written in
// the function that holds it.
#ifdef B
template <class T> int f() { struct L { int a = 2; int b; }; return L{}.a; }
#else
template <class T> int f() { struct L { int a = 1; int b; }; return L{}.a; }
#endif
int x = f<int>();
