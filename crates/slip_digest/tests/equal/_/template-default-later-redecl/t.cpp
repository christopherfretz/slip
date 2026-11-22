#include "side.h"
// spec/digest-kinds.md §9.2 / §9.5 `defaults`: a default template argument
// given by a later redeclaration is the template's, as if the first had given
// it (WP-029: the parameter list written is the one that gives the default).
#ifdef B
template <class T> struct X {};
template <class T = int> struct X;
#else
template <class T = int> struct X {};
#endif
