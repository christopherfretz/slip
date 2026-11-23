namespace n { template <class T> struct S { void f(); }; }
class X { template <class T> friend void n::S<T>::f(); };
