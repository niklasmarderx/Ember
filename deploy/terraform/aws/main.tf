# Ember AWS Infrastructure
# Terraform configuration for deploying Ember on AWS EKS

terraform {
  required_version = ">= 1.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
    kubernetes = {
      source  = "hashicorp/kubernetes"
      version = "~> 2.0"
    }
    helm = {
      source  = "hashicorp/helm"
      version = "~> 2.0"
    }
  }
}

provider "aws" {
  region = var.aws_region
}

# VPC for EKS
module "vpc" {
  source  = "terraform-aws-modules/vpc/aws"
  version = "~> 5.0"

  name = "${var.cluster_name}-vpc"
  cidr = var.vpc_cidr

  azs             = var.availability_zones
  private_subnets = var.private_subnets
  public_subnets  = var.public_subnets

  enable_nat_gateway   = true
  single_nat_gateway   = var.environment == "dev"
  enable_dns_hostnames = true
  enable_dns_support   = true

  public_subnet_tags = {
    "kubernetes.io/role/elb"                    = 1
    "kubernetes.io/cluster/${var.cluster_name}" = "shared"
  }

  private_subnet_tags = {
    "kubernetes.io/role/internal-elb"           = 1
    "kubernetes.io/cluster/${var.cluster_name}" = "shared"
  }

  tags = var.tags
}

# EKS Cluster
module "eks" {
  source  = "terraform-aws-modules/eks/aws"
  version = "~> 19.0"

  cluster_name    = var.cluster_name
  cluster_version = var.kubernetes_version

  vpc_id     = module.vpc.vpc_id
  subnet_ids = module.vpc.private_subnets

  cluster_endpoint_public_access = true

  eks_managed_node_groups = {
    ember = {
      name           = "ember-nodes"
      instance_types = var.node_instance_types
      min_size       = var.node_min_size
      max_size       = var.node_max_size
      desired_size   = var.node_desired_size

      labels = {
        Environment = var.environment
        Application = "ember"
      }

      tags = var.tags
    }
  }

  tags = var.tags
}

# Configure Kubernetes provider
provider "kubernetes" {
  host                   = module.eks.cluster_endpoint
  cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)
  exec {
    api_version = "client.authentication.k8s.io/v1beta1"
    command     = "aws"
    args        = ["eks", "get-token", "--cluster-name", var.cluster_name]
  }
}

# Configure Helm provider
provider "helm" {
  kubernetes {
    host                   = module.eks.cluster_endpoint
    cluster_ca_certificate = base64decode(module.eks.cluster_certificate_authority_data)
    exec {
      api_version = "client.authentication.k8s.io/v1beta1"
      command     = "aws"
      args        = ["eks", "get-token", "--cluster-name", var.cluster_name]
    }
  }
}

# Namespace
resource "kubernetes_namespace" "ember" {
  metadata {
    name = var.namespace
    labels = {
      "app.kubernetes.io/name"    = "ember"
      "app.kubernetes.io/part-of" = "ember"
    }
  }
}

# Deploy Ember using Helm
resource "helm_release" "ember" {
  name       = "ember"
  namespace  = kubernetes_namespace.ember.metadata[0].name
  chart      = "${path.module}/../../../helm/ember"
  
  values = [
    yamlencode({
      api = {
        replicaCount = var.api_replicas
        resources = {
          requests = {
            memory = var.api_memory_request
            cpu    = var.api_cpu_request
          }
          limits = {
            memory = var.api_memory_limit
            cpu    = var.api_cpu_limit
          }
        }
      }
      ingress = {
        enabled = true
        hosts = [{
          host = var.domain
          paths = [{
            path     = "/"
            pathType = "Prefix"
            service  = "web"
          }]
        }, {
          host = "api.${var.domain}"
          paths = [{
            path     = "/"
            pathType = "Prefix"
            service  = "api"
          }]
        }]
        tls = [{
          secretName = "ember-tls"
          hosts      = [var.domain, "api.${var.domain}"]
        }]
      }
      secrets = {
        openaiApiKey    = var.openai_api_key
        anthropicApiKey = var.anthropic_api_key
      }
    })
  ]
}

# S3 bucket for backups (optional)
resource "aws_s3_bucket" "ember_backups" {
  count  = var.enable_backups ? 1 : 0
  bucket = "${var.cluster_name}-backups"
  tags   = var.tags
}

resource "aws_s3_bucket_versioning" "ember_backups" {
  count  = var.enable_backups ? 1 : 0
  bucket = aws_s3_bucket.ember_backups[0].id
  versioning_configuration {
    status = "Enabled"
  }
}

# Outputs
output "cluster_endpoint" {
  description = "EKS cluster endpoint"
  value       = module.eks.cluster_endpoint
}

output "cluster_name" {
  description = "EKS cluster name"
  value       = module.eks.cluster_name
}

output "vpc_id" {
  description = "VPC ID"
  value       = module.vpc.vpc_id
}

output "kubeconfig_command" {
  description = "Command to update kubeconfig"
  value       = "aws eks update-kubeconfig --region ${var.aws_region} --name ${var.cluster_name}"
}