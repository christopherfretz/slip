// WP-112 (digest §9): sizeof(__PRETTY_FUNCTION__) read at compile time in a function template; the parameter type requalified as written changes the instantiation's string (53 to 55 bytes): its definition differs, the pattern and u do not
namespace ns { struct S { static int value; }; }
template <class T> unsigned long h(T, decltype(ns::S::value)) { return sizeof(__PRETTY_FUNCTION__); }
unsigned long u = h(1, 1);
