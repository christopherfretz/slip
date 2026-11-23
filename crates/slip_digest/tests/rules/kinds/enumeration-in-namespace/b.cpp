namespace legacy {
enum E { A, B };
enum class F : char { C };
}
int use() { return legacy::B + (int)legacy::F::C; }
