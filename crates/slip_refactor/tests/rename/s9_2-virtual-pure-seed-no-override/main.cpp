namespace n {
struct B { virtual void f() = 0; };
}
void h(n::B& b) { b.f(); }
