namespace n { template <class T> struct S { void f() {} };
template <class T> struct S<T*> { void f() {} }; }
void h() { n::S<int*> a; a.f(); }
