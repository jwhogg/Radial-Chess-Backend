![badge](https://img.shields.io/badge/version-0.8-yellow)
![badge](https://img.shields.io/badge/project%20status-alpha%3A%20under%20development-blue)

# Radial Chess
<img width="60%" alt="Screenshot 2024-10-25 at 14 31 29" src="https://github.com/user-attachments/assets/eeb29645-a658-4d88-ac06-09dfc15afd10">

A scalable, lightweight, and secure chess app with online matchmaking—written in Rust.

<img width="30%" alt="Demo of a game" src="https://github.com/user-attachments/assets/65dc3ed1-da70-4d31-a273-349de6703df2">

## Table of Contents
- [Introduction](#introduction)
- [Back-end Technologies](#back-end-technologies)
- [Front-end Technologies](#front-end-technologies)
- [Architecture](#architecture)
- [Current State of the Project](#current-state-of-the-project)

## Introduction

Inspired by the [heorics](https://www.youtube.com/watch?v=7VSVfQcaxFY) of [lichess.org](https://lichess.org/)'s single developer, I decided to try to create a similar web-based online chess app, with matchmaking. The main goal of this project is to use my knowledge of system design to make a robust and scalable app that could theoretically handle a large number of users. This involves knowlege of infastructure tools, and overcoming dificulties such as [scaling a Web-Socket app](https://ably.com/topic/the-challenge-of-scaling-websockets) (hint: you will need sticky sessions for your load-balancer!). It was also an opertunity to further my rust education- a language I've been learning in my spare time over the past few months.

## Back-end Technologies
- [Rust 🦀](https://www.rust-lang.org/) for the entire back-end
- [tokio](https://tokio.rs/) and [axum](https://github.com/tokio-rs/axum) for async and networking
- [Redis](https://redis.io/) for scalable state memory
- [JSON Web Token](https://jwt.io/introduction) for secure authentication
- [MySQL](https://www.mysql.com/) for storing persistent user data

## Front-end Technologies
- [VueJs](https://vuejs.org/) for front-end + WebSocket client
- [Vue-axios](https://www.npmjs.com/package/vue-axios) for API consumption
- [Auth0](https://auth0.com/) for secure authentication and Single Sign-On (Google accounts)
- [Vue3-Chessboard](https://github.com/qwerty084/vue3-chessboard) for chess GUI, built on Lichess

## Architecture
*Note: The project is yet to be hosted as it is under development, but these will be the technologies used:*
- [CloudFront](https://aws.amazon.com/cloudfront/) for serving the static site
- [AWS EC2](https://aws.amazon.com/ec2/) for hosting the back-end servers
- [Docker](https://www.docker.com/) for containerization
- [NGINX](https://www.f5.com/go/product/welcome-to-nginx) for load balancing
- [Redis](https://redis.io/) for state management and pub/sub

## Current State of the Project
- [x] Authentication
- [x] Matchmaking
- [x] WebSocket communication
- [ ] Validating and syncing game state across players
- [ ] Handling networking edge cases
- [ ] Testing with >2 concurrent players
- [ ] Productionizing and hosting on cloud infrastructure
