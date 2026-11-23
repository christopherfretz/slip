template <class T> struct S { int g() { return 1; } };
template <> int S<int>::g();
int u() { return S<int>().g(); }
template <> int S<int>::g() { return 2; }
