namespace n { template <class T> struct S { struct B { void f() {} }; }; }
void h() { n::S<int>::B b; b.f(); }
