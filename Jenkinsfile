pipeline {
  agent {
    node {
      label 'rust&&sgx'
    }
  }
  stages {
    stage('Environment') {
      steps {
        sh './ci/install_rust.sh'
      }
    }
    stage('Build') {
      steps {
        sh 'make'
      }
    }
    stage('Test') {
      steps {
        sh 'cd client  && cargo test 2>&1 | tee ${WORKSPACE}/test_client.log'
        sh 'cd worker  && cargo test 2>&1 | tee ${WORKSPACE}/test_worker.log'
        sh 'cd enclave && cargo test 2>&1 | tee ${WORKSPACE}/test_enclave.log'
      }
    }
    stage('Lint') {
      steps {
        sh 'cd client  && cargo +nightly-2019-11-17 clippy 2>&1 | tee ${WORKSPACE}/clippy_client.log'
        sh 'cd worker  && cargo +nightly-2019-11-17 clippy 2>&1 | tee ${WORKSPACE}/clippy_worker.log'
        sh 'cd enclave && cargo +nightly-2019-11-17 clippy 2>&1 | tee ${WORKSPACE}/clippy_enclave.log'
      }
    }
    stage('Archive build output') {
      steps {
        archiveArtifacts artifacts: '*.log'
      }
    }
  }
  post {
    always {
      recordIssues(
        enabledForFailure: true,
        aggregatingResults: true,
        qualityGates: [
          [ threshold: 1, type: 'TOTAL', unstable: true ]
        ],
        tools: [
          groovyScript(parserId:'clippy-warnings', pattern:'**/clippy_*.log', reportEncoding:'UTF-8'),
        ]
      )
    }
  }
}
