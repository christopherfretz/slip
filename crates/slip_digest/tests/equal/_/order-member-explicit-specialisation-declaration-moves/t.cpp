#include "side.h"
// [temp.expl.spec]/14: moving the declaration `template <> int A<char>::s;` past another definition moves no definition (WP-033 review, item 2).
template <class T> struct A { static int s; };
#ifdef B
int g = 2; template <> int A<char>::s;
#else
template <> int A<char>::s; int g = 2;
#endif
template <> int A<char>::s = 3;
