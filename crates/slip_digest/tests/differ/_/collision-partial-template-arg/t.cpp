#include "side.h"
// digest §3.4: partial specialisations differing by a template template
// argument are two keys.
template <class T> struct A {};
template <class T> struct R {};
template <class T> struct X {};
#ifdef B
template <class T> struct X<A<T>> {};
template <class T> struct X<R<T>> {};
#else
template <class T> struct X<A<T>> {};
#endif
