#include "side.h"
// WP-125 item 3 (digest §9): std::source_location::current().function_name()
// in a function template with one instantiation, its length consumed as an
// array bound; the parameter type respelled with a leading `::` (clang-19:
// the bound is 52 against 54). The bound is a type, written by value in the
// instantiation's definition; the pattern writes the value Kiln records
// at the use there (its own name), so it differs too.
#include "srcloc.h"
namespace ns { struct S { static int value; }; }
constexpr unsigned len(const char *s) { unsigned n = 0; while (s[n]) ++n; return n; }
#ifdef B
template <class T> unsigned long h(T, decltype(::ns::S::value)) { char x[len(std::source_location::current().function_name())]; return sizeof x; }
#else
template <class T> unsigned long h(T, decltype(ns::S::value)) { char x[len(std::source_location::current().function_name())]; return sizeof x; }
#endif
unsigned long use() { return h(1, 1); }
