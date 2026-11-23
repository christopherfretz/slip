namespace n { template <class T> struct S { void f() {} }; }
void h() { n::S<int> a; a.f(); n::S<long> b; b.f(); }
