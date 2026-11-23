namespace n { template <class T> struct S { void f() {} }; }
template <class T> auto pm() { return &n::S<T>::f; }
