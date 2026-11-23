namespace acme {
struct S {};
template <class T> int f(T) { return 1; }
template <> int f(S) { return 2; }
}
int use() { return acme::f(1) + acme::f(acme::S()); }
