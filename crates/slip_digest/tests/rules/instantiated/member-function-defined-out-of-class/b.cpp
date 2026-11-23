template <class T> struct S { int g(); };
template <class T> int S<T>::g() { return 1; }
int u() { return S<int>().g(); }
