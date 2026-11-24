// WP-112 (digest §9): __PRETTY_FUNCTION__ passed on in a function template (as assert does); the parameter type requalified as written changes the instantiation's string
namespace ns { struct S { static int value; }; }
void take(const char *);
template <class T> void h(T, decltype(ns::S::value)) { take(__PRETTY_FUNCTION__); }
void u() { h(1, 1); }
