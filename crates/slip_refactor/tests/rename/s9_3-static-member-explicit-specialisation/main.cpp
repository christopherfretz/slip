namespace n { template <class T> struct S { static int v; };
template <class T> int S<T>::v = 0;
template <> int S<int>::v = 1; }
int k = n::S<int>::v + n::S<long>::v;
