namespace n { template <class T> struct S { void f() {} }; }
template <class T> concept C = requires(n::S<T> s) { s.f(); &n::S<T>::f; };
