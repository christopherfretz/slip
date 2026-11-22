#include "side.h"
// [temp.expl.spec]/14: `template <> int A<char>::s;` declares; the order unit lists the definition where it is (WP-033 review, item 2).
int h(); template <class T> struct A { static int s; }; template <> int A<char>::s;
#ifdef B
template <> int A<char>::s = h(); int g = h();
#else
int g = h(); template <> int A<char>::s = h();
#endif
