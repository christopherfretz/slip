template <class T> struct S { int g() { return 1; } };
template <> int S<int>::g() { return 2; }
int u() { return S<int>().g(); }
