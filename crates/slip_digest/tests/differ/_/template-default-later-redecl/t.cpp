#include "side.h"
// §9.5 `defaults` from a later redeclaration: changing it changes the
// template.
#ifdef B
template <class T> struct X {};
template <class T = long> struct X;
#else
template <class T> struct X {};
template <class T = int> struct X;
#endif
