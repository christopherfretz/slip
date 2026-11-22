#include "side.h"
// §2.3: the unit of a member explicit specialisation is read from its definition, the declaration with the initialiser, not from `template <> int A<char>::s;` (WP-033 review, item 2).
template <class T> struct A { static int s; }; template <> int A<char>::s;
#ifdef B
template <> int A<char>::s = 2;
#else
template <> int A<char>::s = 1;
#endif
