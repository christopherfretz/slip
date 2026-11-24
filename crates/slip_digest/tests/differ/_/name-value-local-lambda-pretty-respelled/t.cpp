#include "side.h"
// WP-125 item 3, neighbour found (digest §9): the same as
// name-value-template-lambda-pretty-respelled in a function that is not a
// template. clang-19: "auto g(decltype(ns::S::value))::(anonymous
// class)::operator()() const", 70 bytes against 72, so g differs. Kiln
// prints "auto g(int)::(lambda)::operator()() const" on both sides (For
// Kiln, spec/landed/digest/WP-125.md).
namespace ns { struct S { static int value; }; }
#ifdef B
unsigned long g(decltype(::ns::S::value)) { return [] { return sizeof(__PRETTY_FUNCTION__); }(); }
#else
unsigned long g(decltype(ns::S::value)) { return [] { return sizeof(__PRETTY_FUNCTION__); }(); }
#endif
