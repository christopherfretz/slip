namespace __attribute__((visibility("hidden"))) o { namespace __attribute__((type_visibility("default"))) i { struct S { virtual ~S(); }; } }
namespace __attribute__((visibility("hidden"), type_visibility("default"))) p { }
o::i::S::~S() {}
