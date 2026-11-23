namespace n { template <class T> struct S { void f(T&); }; }
class X { int p; template <class T> friend void n::S<T>::f(T&); };
template <class T> void n::S<T>::f(T& t) { (void)t.p; }
