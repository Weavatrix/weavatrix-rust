mod language_fixture;

use blazingly_json::json;
use language_fixture::Fixture;
use weavatrix_rust::{Weavatrix, tools};

fn dependent_labels(report: &blazingly_json::Value) -> Vec<String> {
    report["dependents"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["node"]["label"].as_str().map(str::to_owned))
        .collect()
}

#[test]
fn nestjs_constructor_injection_is_visible_in_the_blast_radius() {
    let fixture = Fixture::new();
    fixture.write(
        "src/users/users.service.ts",
        "import { Injectable } from '@nestjs/common';\n\n\
         @Injectable()\n\
         export class UsersService {\n\
           findAll(): string[] {\n\
             return ['alice'];\n\
           }\n\
         }\n",
    );
    fixture.write(
        "src/users/users.controller.ts",
        "import { Controller, Get } from '@nestjs/common';\n\
         import { UsersService } from './users.service';\n\n\
         @Controller('users')\n\
         export class UsersController {\n\
           constructor(private readonly usersService: UsersService) {}\n\n\
           @Get()\n\
           findAll(): string[] {\n\
             return this.usersService.findAll();\n\
           }\n\
         }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    let report = tools::call(
        &mut engine,
        "get_dependents",
        json!({"label": "UsersService", "depth": 2}),
    )
    .unwrap();
    let labels = dependent_labels(&report);
    assert!(
        labels.iter().any(|label| label == "UsersController"),
        "constructor injection couples the controller to its provider: {report:?}"
    );
}

#[test]
fn spring_field_and_constructor_injection_are_visible_in_the_blast_radius() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main/java/com/example/OrderService.java",
        "package com.example;\n\nimport org.springframework.stereotype.Service;\n\n\
         @Service\npublic class OrderService {\n\
         \x20   public String list() {\n\
         \x20       return \"orders\";\n\
         \x20   }\n\
         }\n",
    );
    fixture.write(
        "src/main/java/com/example/AuditService.java",
        "package com.example;\n\nimport org.springframework.stereotype.Service;\n\n\
         @Service\npublic class AuditService {\n\
         \x20   public void record(String event) {}\n\
         }\n",
    );
    fixture.write(
        "src/main/java/com/example/OrderController.java",
        "package com.example;\n\n\
         import org.springframework.beans.factory.annotation.Autowired;\n\
         import org.springframework.web.bind.annotation.GetMapping;\n\
         import org.springframework.web.bind.annotation.RestController;\n\n\
         @RestController\npublic class OrderController {\n\n\
         \x20   private final OrderService orderService;\n\n\
         \x20   @Autowired\n\
         \x20   private AuditService auditService;\n\n\
         \x20   @Autowired\n\
         \x20   public OrderController(OrderService orderService) {\n\
         \x20       this.orderService = orderService;\n\
         \x20   }\n\n\
         \x20   @GetMapping(\"/orders\")\n\
         \x20   public String orders() {\n\
         \x20       auditService.record(\"orders\");\n\
         \x20       return orderService.list();\n\
         \x20   }\n\
         }\n",
    );
    let mut engine = Weavatrix::open(&fixture.root).unwrap();

    for service in ["OrderService", "AuditService"] {
        let report = tools::call(
            &mut engine,
            "get_dependents",
            json!({"label": service, "depth": 2}),
        )
        .unwrap();
        let labels = dependent_labels(&report);
        assert!(
            labels.iter().any(|label| label == "OrderController"),
            "{service} must list the controller that injects it: {report:?}"
        );
    }
}
