template <class T> struct S { template <class U> int h(U); };
template <class T> template <class U> int S<T>::h(U) { return 1; }
int u() { return S<int>().h(1); }
