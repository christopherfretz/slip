namespace n { template <class T> struct S { static int v; }; template <class T> int S<T>::v = 0; }
int h() { return n::S<int>::v; }
