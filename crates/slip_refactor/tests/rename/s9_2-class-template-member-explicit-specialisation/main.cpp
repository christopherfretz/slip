namespace n { template <class T> struct S { void f() {} };
template <> void S<int>::f() {} }
void h() { n::S<int> s; s.f(); }
