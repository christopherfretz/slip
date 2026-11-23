namespace n {
struct B { virtual void f(); void f(int); };
struct D : B { using B::f; void f() override; };
}
