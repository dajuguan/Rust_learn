# self-reference结构除了async也存在，为什么只有futures必须限定Pin<&mut self>？
- 因为async的代码，编译器会自动生成自引用代码，开发者无法自行控制，所以需要在编译器层面保证安全，其他则由开发者自行保证安全？
- poll如何被调用？