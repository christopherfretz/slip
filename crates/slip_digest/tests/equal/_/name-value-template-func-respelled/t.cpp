#include "side.h"
// WP-125 item 3 (digest §9): sizeof(__func__) in a function template with
// one instantiation whose parameter type is respelled with a leading `::`:
// `__func__` holds the unqualified name only ("h", 2 bytes on both sides,
// clang-19), so every unit is equal and so is the names digest.
namespace ns { struct S { static int value; }; }
#ifdef B
template <class T> unsigned long h(T, decltype(::ns::S::value)) { return sizeof(__func__); }
#else
template <class T> unsigned long h(T, decltype(ns::S::value)) { return sizeof(__func__); }
#endif
unsigned long use() { return h(1, 1); }
