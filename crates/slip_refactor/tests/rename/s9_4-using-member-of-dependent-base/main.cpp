namespace n { template <class T> struct S { void f() {} }; }
template <class T> struct D : n::S<T> { using n::S<T>::f; };
