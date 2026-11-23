namespace n { template <class T> struct S { void f(); void h(); }; }
class X { template <class T> friend void n::S<T>::h(); };
