#include "side.h"
// WP-125 item 3 (digest §9): a lambda's __PRETTY_FUNCTION__ inside a
// function template with one instantiation, its length consumed; the
// template's parameter type respelled with a leading `::`. clang-19 prints
// the enclosing function's parameters as written, "auto h(int,
// decltype(ns::S::value))::(anonymous class)::operator()() const" (75 bytes
// against 77), so the instantiation's definition (which writes the closure)
// differs. Kiln prints "auto h(int, int)::(lambda)::operator()() const" on
// both sides: the respelling is lost (For Kiln, spec/landed/digest/WP-125.md).
namespace ns { struct S { static int value; }; }
#ifdef B
template <class T> unsigned long h(T, decltype(::ns::S::value)) { return [] { return sizeof(__PRETTY_FUNCTION__); }(); }
#else
template <class T> unsigned long h(T, decltype(ns::S::value)) { return [] { return sizeof(__PRETTY_FUNCTION__); }(); }
#endif
unsigned long use() { return h(1, 1); }
