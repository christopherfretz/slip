template <class T> struct S { int g() { return 1; } };
int u() { return S<int>().g(); }
