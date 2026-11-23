namespace n { template <class T> struct S { void f() {} };
template <> struct S<int> { void f() {} }; }
void h() { n::S<int> a; a.f(); }
