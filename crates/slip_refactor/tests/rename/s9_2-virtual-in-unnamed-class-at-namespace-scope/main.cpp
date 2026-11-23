namespace n { struct B { virtual void f(); }; }
struct : n::B { void f() override {} } u;
