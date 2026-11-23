namespace n { template <class T> struct S { void f() {} }; }
void h() { n::S<int> s; s.f(); }
