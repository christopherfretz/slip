namespace n { template <class T> struct A { struct B { static void f() {} }; }; }
void h() { n::A<int>::B::f(); }
