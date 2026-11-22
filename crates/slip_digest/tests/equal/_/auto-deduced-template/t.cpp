// §4.7 TypeView `Auto.deduced`: a templated function's placeholder deduced as dependent is determinate, and
// so is a member's of a class template.
template <class T> auto g(T t) { return t; }
template <class T> struct S { auto m() { return T(); } };
