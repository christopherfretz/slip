namespace acme {
enum E { A, B };
enum class F : char { C };
}
int use() { return acme::B + (int)acme::F::C; }
