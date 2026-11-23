template <class T> struct S { static int w; };
template <class T> int S<T>::w = 1;
int u() { return S<int>::w; }
