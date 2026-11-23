namespace n {
struct B { virtual void f(); };
struct D : B { using B::f; void f() override; };
}
