namespace n { template <class T> struct S { template <class U> void f(U) {} }; }
void h() { n::S<int> s; s.f(1); s.f<long>(2); s.template f<char>(3); }
