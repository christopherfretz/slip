namespace acme {
struct S { int v = 1; };
template <class T> int f(T t = T()) { return t.v; }
}
int use() { return acme::f<acme::S>(); }
