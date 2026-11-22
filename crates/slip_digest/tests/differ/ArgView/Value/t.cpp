#include "side.h"
// §4.6 ArgView `Value`: a value argument against a type argument (the template's head differs too, so A changes as well).
#ifdef B
template <auto V> struct A;
A<1>* x;
#else
template <class T> struct A;
A<int>* x;
#endif
