namespace n { template <class T> struct S { void f() {} void k() { f(); this->f(); } }; }
void h() { n::S<int> s; s.f(); s.n::S<int>::f(); }
